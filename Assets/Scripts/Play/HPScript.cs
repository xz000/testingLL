﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class HPScript : MonoBehaviour
{
    public float maxHP;
    public float currentHP;
    public Sender ser;
    private GameObject safeground;
    float outhurt = 2;
    bool boost = false;
    public float boostnow = 0;
    public float boostmax = 25;

    // Use this for initialization
    void Start()
    {
        currentHP = maxHP;
        safeground = GameObject.FindGameObjectWithTag("Ground");
        boost = false;
    }

    // Update is called once per frame

    private void FixedUpdate()
    {
        if (transform.position.magnitude > safeground.GetComponent<AreaScript>().radius)
        {
            currentHP -= outhurt * Time.fixedDeltaTime;
        }
        if (currentHP <= 0)
        {
            GetComponent<DoSkill>().DoClearJob();
            GetComponent<DoSkill>().DestroyClean();
            GetComponent<DestroyScript>().Destroyself();
        }
        if (currentHP > maxHP)
        {
            currentHP = maxHP;
        }
    }

    private void OnDestroy()
    {
        if (!gameObject.CompareTag("Player"))
            return;
        GameObject[] PlayersLeft = GameObject.FindGameObjectsWithTag("Player");
        Debug.Log(PlayersLeft.Length + "Players Left Now.");
        if (PlayersLeft.Length == 1 && ser != null)
        {
            ser.RealEnd();
            EndData endData = new EndData();
            endData.CircleID = int.Parse(gameObject.name);
            endData.epx = GetComponent<Rigidbody2D>().position.x;
            endData.epy = GetComponent<Rigidbody2D>().position.y;
            ser.SendEnd(endData);
            Debug.Log("awsl");
            if (Sender.isTesting)
            {
                ser.RealEnd();
                ser.SendMessage("BattlesFinish");
            }
        }
    }


    public void TransferTo(Vector2 destination)
    {
        GetComponent<Rigidbody2D>().position = destination;
    }

    public void GetHurt(float damage)
    {
        currentHP -= damage;
        if (boost)
        {
            float halfdamage = damage / 2;
            float boostvalue = boostmax - boostnow;
            if (boostmax - boostnow >= halfdamage)
                boostvalue = halfdamage;
            currentHP += boostvalue;
            boostnow += boostvalue;
            GetComponent<MoveScript>().movespeed += (float)boostvalue / 50;
        }
    }

    public void boostend()
    {
        boost = false;
        GetComponent<MoveScript>().movespeed -= (float)boostnow / 50;
        boostnow = 0;
    }

    public void booststart()
    {
        boostend();
        boost = true;
    }

    /*public void GetKicked(Vector2 force)
    {
        GetComponent<Rigidbody2D>().AddForce(force);
    }*/
}
