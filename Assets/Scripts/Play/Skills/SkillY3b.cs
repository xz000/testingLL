using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillY3b : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public GameObject TheStar;
    public float maxdistance = 6;
    public float powerpersecond = 2;
    public float maxtime = 4;
    private float currentcooldown;
    public float cooldowntime = 10;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillY3b()
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
        GetComponent<DoSkill>().BeforeSkill();
        Fix64 mdfx = (Fix64)maxdistance;
        if (skilldirection.Length() > mdfx)
            actionplace = singplace + skilldirection.normalized() * mdfx;
        GameObject MyRock = Instantiate(TheStar, actionplace.ToV2(), Quaternion.identity);
        MyRock.GetComponent<StarScript>().sender = gameObject;
        MyRock.GetComponent<StarScript>().powerpers = powerpersecond;
        MyRock.GetComponent<CountdownScript>().maxtime = maxtime;
        currentcooldown = 0;
        skillavaliable = false;
    }

    void SkillY3bSetLevel(int i)
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
            GameObject.Find("Canvas2").GetComponent<BottomLink>().iY.Fif = CalcFA;
    }
}
