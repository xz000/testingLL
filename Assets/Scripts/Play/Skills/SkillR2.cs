using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillR2 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float SpeedR2;
    public float maxdistance;
    public float pushPower;
    public float pushTime;
    public float pushDamage;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    MoveScript MS;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
        MS = GetComponent<MoveScript>();
    }

    // Update is called once per frame
    void GoSkillR2()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
        }
    }

    public void Skill(Fix64Vector2 actionplace)
    {
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        Fix64Vector2 skilldirection = actionplace - singplace;
        float realdistance = Mathf.Min((float)skilldirection.Length(), maxdistance);
        if (realdistance <= 0.6)
        {
            return;
        }   //半径小于自身半径时不施法
        else
        {
            GetComponent<DoSkill>().BeforeSkill();
            MS.controllable = true;
            currentcooldown = 0;
            skillavaliable = false;
            float TimeR2 = (float)((Fix64)realdistance / (Fix64)SpeedR2);
            gameObject.GetComponent<ColliderScript>().SetPower(pushPower, pushTime, pushDamage);
            gameObject.GetComponent<ColliderScript>().StartKick(TimeR2);
            gameObject.GetComponent<RBScript>().GetPushed(skilldirection.normalized() * (Fix64)SpeedR2, TimeR2);
        }
    }

    void SkillR2SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }

    void LinkToIcon()
    {
        if (enabled)
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iR.Fif = CalcFA;
    }
}
