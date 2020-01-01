﻿using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillC3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject ShadowCircle;
    GameObject MyShadow;
    private float currentcooldown;
    public float cooldowntime = 3;
    public float maxshadowtime = 2.5f;
    //float shadowtime;
    public bool skillavaliable;
    bool HaveShadow = false;
    bool canctrl;
    Fix64Vector2 flyspeed;
    float remaintime;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        //ShadowCircle.GetComponent<CircleCollider2D>().enabled = false;
        //ShadowCircle.GetComponent<SpriteRenderer>().color = new Color32(255, 255, 255, 127);
    }

    public void GoSkillC3()
    {
        if (HaveShadow)
        {
            BackToShadow();
        }
        else if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            Skill();
        }
    }

    private void FixedUpdate()
    {
        if (!skillavaliable)
        {
            currentcooldown += Time.fixedDeltaTime;
            if (HaveShadow && currentcooldown >= maxshadowtime)
                BackToShadow();
            if (currentcooldown >= cooldowntime)
            {
                skillavaliable = true;
            }
        }
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    void Skill()
    {
        //shadowtime = 0;
        GetComponent<DoSkill>().BeforeSkill();
        MyShadow = Instantiate(ShadowCircle, transform.position, Quaternion.identity);
        if (GetComponent<MoveScript>().isme)
            MyShadow.GetComponent<ShadowControl>().ShowSelf();
        canctrl = gameObject.GetComponent<MoveScript>().controllable;
        if (!canctrl)
        {
            flyspeed = gameObject.GetComponent<MoveScript>().Givenvelocity;
            remaintime = gameObject.GetComponent<RBScript>().GetRemainTime();
        }
        currentcooldown = 0;
        skillavaliable = false;
        HaveShadow = true;
        gameObject.GetComponent<DoSkill>().WorkBeforeDestroy += DestroyMyShadow;
    }

    void BackToShadow()
    {
        gameObject.GetComponent<MoveScript>().stopwalking(); //停止走动
        transform.position = MyShadow.transform.position;
        gameObject.GetComponent<MoveScript>().controllable = canctrl;
        if (!canctrl)
        {
            gameObject.GetComponent<RBScript>().GetPushed(flyspeed, remaintime);
        }
        HaveShadow = false;
        GameObject.Destroy(MyShadow);
        gameObject.GetComponent<DoSkill>().WorkBeforeDestroy -= DestroyMyShadow;
        gameObject.GetComponent<DoSkill>().DoClearJob();
    }

    public void DestroyMyShadow()
    {
        if (MyShadow == null)
            return;
        GameObject.Destroy(MyShadow);
    }

    void SkillC3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iC.Fif = CalcFA;
    }
}
