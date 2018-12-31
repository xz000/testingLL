using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class HPScript : MonoBehaviour
{
    public float maxHP;
    public float currentHP;
    //private GameObject safeground;
    float outhurt = 2;
    bool boost = false;
    public float boostnow = 0;
    public float boostmax = 25;

    // Use this for initialization
    void Start () {
        currentHP = maxHP;
        //safeground = GameObject.Find("GroundCircle");
        boost = false;
	}

    // Update is called once per frame

    /*private void FixedUpdate()
    {
        if (transform.position.magnitude > safeground.transform.lossyScale.x / 2)
        {
            currentHP -= outhurt * Time.fixedDeltaTime;
        }
    }*/

    void LateUpdate () {
		if(currentHP <= 0)
        {
            gameObject.GetComponent<DoSkill>().DoClearJob();
            gameObject.GetComponent<DoSkill>().DestroyClean();
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
        if(currentHP > maxHP)
        {
            currentHP = maxHP;
        }
	}

    private void OnDestroy()
    {
        if (!gameObject.CompareTag("Player"))
            return;
        GameObject[] PlayersLeft = GameObject.FindGameObjectsWithTag("Player");
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
            float boostvalue = Mathf.Min(boostmax - boostnow, damage / 2);
            currentHP += boostvalue;
            boostnow += boostvalue;
            GetComponent<MoveScript>().movespeed += boostvalue / 50;
        }
    }

    public void boostend()
    {
        boost = false;
        GetComponent<MoveScript>().movespeed -= boostnow / 50;
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
